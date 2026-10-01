pub type InferenceBackend = burn::backend::Wgpu<f32, i32>;

pub type TrainingBackend = burn::backend::Autodiff<InferenceBackend>;

pub type Device = burn::backend::wgpu::WgpuDevice;

pub fn default_device() -> Device {
    Device::default()
}

#[cfg(test)]
mod backend_tests {
    use super::{TrainingBackend, default_device};
    use burn::tensor::Tensor;

    #[test]
    fn wgpu_backend_runs_forward_and_backward() {
        let device = default_device();
        let input = Tensor::<TrainingBackend, 2>::from_floats([[1.0, 2.0]], &device).require_grad();
        let output = input.clone().matmul(input.clone().transpose());
        let gradients = output.clone().sum().backward();
        let gradient = input.grad(&gradients).expect("input gradient");

        assert_eq!(output.to_data().into_vec::<f32>().unwrap(), vec![5.0]);
        assert_eq!(
            gradient.to_data().into_vec::<f32>().unwrap(),
            vec![2.0, 4.0]
        );
    }
}
